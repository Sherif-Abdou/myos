#pragma once

#include <utility>

template <typename T> class LinkedNode {
  private:
    T item;
    LinkedNode<T> *prev_;
    LinkedNode<T> *next_;

  public:
    template <typename... Args>
    LinkedNode(Args... args)
        : item(std::forward<Args>(args)...), prev_(nullptr), next_(nullptr) {}

    void remove() {
        if (prev_)
            prev_->next_ = next_;
        if (next_)
            next_->prev_ = prev_;
        prev_ = nullptr;
        next_ = nullptr;
    }

    void insert_after(LinkedNode<T> *node) {
        if (node) {
            node->prev_ = this;
            node->next_ = next_;
            if (node->next_)
                node->next_->prev_ = node;
        }
        next_ = node;
    }

    void insert_before(LinkedNode<T> *node) {
        if (node) {
            node->next_ = this;
            node->prev_ = prev_;
            if (node->prev_)
                node->prev_->next_ = node;
        }
        prev_ = node;
    }

    LinkedNode<T> *next() { return next_; }

    const LinkedNode<T> *next() const { return next_; }

    LinkedNode<T> *prev() { return prev_; }

    const LinkedNode<T> *prev() const { return prev_; }

    T &operator*() { return item; }

    const T &operator*() const { return item; }

    T *operator->() { return &item; }

    const T *operator->() const { return &item; }

    ~LinkedNode() { remove(); }
};
